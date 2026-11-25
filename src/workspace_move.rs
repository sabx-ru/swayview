//! Workspace drag: change Sway's real numbered ordering, not just the preview.
use std::collections::HashSet;
use anyhow::{Context, Result, ensure, bail};
use crate::model::{ConId, Tree, Workspace};
use crate::sway::Ipc;

fn locate(tree: &Tree, id: ConId) -> Option<(&str, &Workspace)> {
    tree.outputs.iter().find_map(|o| o.workspaces.iter().find(|w| w.id == id).map(|w| (o.name.as_str(), w)))
}

pub fn same_topology(a: &Tree, b: &Tree) -> bool {
    a.outputs.len() == b.outputs.len() && a.outputs.iter().all(|o| {
        b.output(&o.name).is_some_and(|p| o.rect == p.rect && o.workspaces.iter().map(|w| (w.id, &w.name))
            .eq(p.workspaces.iter().map(|w| (w.id, &w.name))))
    })
}

fn number(w: &Workspace) -> Result<i32> {
    let n = w.num.context("workspace is not numbered")?;
    ensure!(n > 0 && w.name == n.to_string(), "only plain positive numbered workspaces can be reordered");
    Ok(n)
}

fn desired_order(ids: &[ConId], moved: ConId, anchor: ConId, after: bool) -> Result<Vec<ConId>> {
    if moved == anchor { return Ok(ids.to_vec()); }
    let mut result: Vec<_> = ids.iter().copied().filter(|&id| id != moved).collect();
    let pos = result.iter().position(|&id| id == anchor).context("insertion anchor disappeared")?;
    result.insert(pos + if after { 1 } else { 0 }, moved);
    Ok(result)
}

fn rename_plan(tree: &Tree, output: &str, order: &[ConId]) -> Result<Vec<(ConId, String, String)>> {
    let o = tree.output(output).context("output disappeared")?;
    let ids: HashSet<_> = o.workspaces.iter().map(|w| w.id).collect();
    ensure!(order.len() == ids.len() && order.iter().copied().collect::<HashSet<_>>() == ids, "workspace set changed");
    let mut numbers: Vec<_> = o.workspaces.iter().map(number).collect::<Result<_>>()?;
    numbers.sort_unstable();
    ensure!(numbers.windows(2).all(|n| n[0] != n[1]), "duplicate workspace number");
    // Refuse ambiguous numbers on any other monitor as well.
    ensure!(!tree.outputs.iter().filter(|p| p.name != output).flat_map(|p| &p.workspaces)
        .any(|w| w.num.is_some_and(|n| numbers.contains(&n))), "number used on another output");
    Ok(order.iter().zip(numbers).filter_map(|(&id, n)| {
        let w = o.workspaces.iter().find(|w| w.id == id)?;
        let name = n.to_string();
        (w.name != name).then(|| (id, w.name.clone(), name))
    }).collect())
}

fn temporary(tree: &Tree, id: ConId, tag: &str) -> String {
    let mut i = 0;
    loop {
        let name = format!("swayview-{tag}-{}-{}-{i}", std::process::id(), id.0);
        if !tree.workspaces().any(|w| w.name.eq_ignore_ascii_case(&name)) { return name; }
        i += 1;
    }
}

fn rename(ipc: &mut Ipc, id: ConId, old: &str, new: &str) -> Result<()> {
    let tree = ipc.get_tree()?;
    ensure!(locate(&tree, id).is_some_and(|(_, w)| w.name == old), "workspace changed before rename");
    ensure!(!tree.workspaces().any(|w| w.id != id && w.name.eq_ignore_ascii_case(new)), "workspace name already occupied");
    ensure!([old, new].iter().all(|name| !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')), "unsupported workspace name");
    // Only numeric names and internally generated ASCII temporary names reach here.
    ipc.command(&format!("rename workspace {old} to {new}"))?;
    let tree = ipc.get_tree()?;
    ensure!(locate(&tree, id).is_some_and(|(_, w)| w.name == new), "rename was not reflected in Sway");
    Ok(())
}

fn restore_names(ipc: &mut Ipc, plan: &[(ConId, String, String)]) -> Result<()> {
    // Vacate all modified names first so that cyclic renames can be undone.
    let mut staged = Vec::new();
    for (id, old, new) in plan {
        let tree = ipc.get_tree()?;
        let Some((_, w)) = locate(&tree, *id) else { continue; };
        if w.name != *old {
            let prefix = format!("swayview-move-{}-{}-", std::process::id(), id.0);
            ensure!(w.name == *new || w.name.starts_with(&prefix), "workspace renamed externally during operation");
            let tmp = temporary(&tree, *id, "rollback");
            rename(ipc, *id, &w.name, &tmp)?;
            staged.push((*id, tmp, old.clone()));
        }
    }
    for (id, tmp, old) in staged { rename(ipc, id, &tmp, &old)?; }
    Ok(())
}

fn apply_plan(ipc: &mut Ipc, plan: &[(ConId, String, String)]) -> Result<()> {
    let result = (|| -> Result<()> {
        let mut staged = Vec::new();
        for (id, old, new) in plan {
            let tmp = temporary(&ipc.get_tree()?, *id, "move");
            rename(ipc, *id, old, &tmp)?;
            staged.push((*id, tmp, new.clone()));
        }
        for (id, tmp, new) in staged { rename(ipc, id, &tmp, &new)?; }
        Ok(())
    })();
    if let Err(e) = result {
        if let Err(rollback) = restore_names(ipc, plan) {
            bail!("{e:#}; restoring names failed: {rollback:#}");
        }
        return Err(e);
    }
    Ok(())
}

fn move_output(ipc: &mut Ipc, id: ConId, output: &str) -> Result<()> {
    let tree = ipc.get_tree()?;
    let (source, ws) = locate(&tree, id).context("source workspace disappeared")?;
    if source == output { return Ok(()); }
    ensure!(tree.output(output).is_some(), "target output disappeared");
    // An exact window criterion selects the workspace without a speculative
    // workspace switch (which could destroy an empty workspace). Sway 1.10's
    // move workspace command uses the criterion's workspace context.
    let win = ws.windows.iter().find(|w| !w.sticky).context("empty or sticky-only workspace cannot be moved across outputs")?;
    ensure!(output.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)), "unsupported output name");
    ipc.command(&format!("[con_id={}] move workspace to output {output}", win.id))?;
    ensure!(locate(&ipc.get_tree()?, id).is_some_and(|(o, _)| o == output), "workspace did not reach target output");
    Ok(())
}

pub fn execute(ipc: &mut Ipc, displayed: &Tree, moved: ConId, target: &str, anchor: ConId, after: bool) -> Result<()> {
    let before = ipc.get_tree()?;
    ensure!(same_topology(displayed, &before), "workspaces or outputs changed; retry the drag");
    let (source, w) = locate(&before, moved).context("source disappeared")?;
    number(w)?;
    let dest = before.output(target).context("target disappeared")?;
    ensure!(dest.workspaces.iter().any(|w| w.id == anchor), "target workspace disappeared");
    if moved == anchor { return Ok(()); }
    // Validate numbering before making any changes.
    for ws in &dest.workspaces { number(ws)?; }
    let cross = source != target;
    let ids: Vec<_> = dest.workspaces.iter().map(|w| w.id).collect();
    let wanted = desired_order(&ids, moved, anchor, after)?;
    if !cross && wanted == ids { return Ok(()); }
    if cross {
        // Preflight all destination numbers including the incoming one.
        let mut projected = before.clone();
        let incoming = w.clone();
        projected.outputs.iter_mut().find(|o| o.name == source).unwrap().workspaces.retain(|w| w.id != moved);
        projected.outputs.iter_mut().find(|o| o.name == target).unwrap().workspaces.push(incoming);
        rename_plan(&projected, target, &wanted)?;
    }
    let result = (|| -> Result<()> {
        if cross { move_output(ipc, moved, target)?; }
        let now = ipc.get_tree()?;
        let dest_now = now.output(target).context("target disconnected")?;
        // Sway may destroy the formerly visible empty target workspace.
        // Preserve the relative insertion position among all surviving IDs.
        let survivors: HashSet<_> = dest_now.workspaces.iter().map(|w| w.id).collect();
        ensure!(survivors.contains(&moved), "moved workspace disappeared");
        for old in &dest.workspaces {
            ensure!(survivors.contains(&old.id) || old.windows.is_empty(), "nonempty target workspace disappeared");
        }
        let order: Vec<_> = wanted.iter().copied().filter(|id| survivors.contains(id)).collect();
        let plan = rename_plan(&now, target, &order)?;
        apply_plan(ipc, &plan)?;
        Ok(())
    })();
    if let Err(e) = result {
        if cross && let Err(rollback) = move_output(ipc, moved, source) {
            bail!("{e:#}; returning workspace to original output failed: {rollback:#}");
        }
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insertion_forward_backward_and_noop() {
        let (a,b,c) = (ConId(1),ConId(2),ConId(3));
        let ids = [a,b,c];
        assert_eq!(desired_order(&ids,c,a,false).unwrap(), [c,a,b]);
        assert_eq!(desired_order(&ids,a,c,true).unwrap(), [b,c,a]);
        assert_eq!(desired_order(&ids,b,a,true).unwrap(), ids);
        assert_eq!(desired_order(&ids,b,b,false).unwrap(), ids);
        assert_eq!(desired_order(&[a,b],c,b,false).unwrap(), [a,c,b]);
        assert!(desired_order(&ids,a,ConId(99),true).is_err());
    }
    #[test]
    fn cross_output_insertion_and_duplicate_numbers() {
        let mut t = crate::model::tests::tree();
        let mut second = t.outputs[0].clone();
        second.name = "DP-7".into();
        second.workspaces.truncate(1);
        second.workspaces[0].id = ConId(9000);
        second.workspaces[0].name = "9".into();
        second.workspaces[0].num = Some(9);
        let moved = t.outputs[0].workspaces.pop().unwrap();
        let anchor = second.workspaces[0].id;
        let order = desired_order(&[anchor], moved.id, anchor, false).unwrap();
        second.workspaces.push(moved.clone());
        t.outputs.push(second);
        assert!(rename_plan(&t, "DP-7", &order).unwrap().is_empty());
        let order = desired_order(&[anchor], moved.id, anchor, true).unwrap();
        assert_eq!(rename_plan(&t, "DP-7", &order).unwrap().len(), 2);
        t.outputs[0].workspaces[0].num = Some(9);
        assert!(rename_plan(&t, "DP-7", &order).is_err());
    }
    #[test]
    fn numbering_preserves_existing_slots() {
        let mut t = crate::model::tests::tree();
        let o = &mut t.outputs[0];
        for (w,n) in o.workspaces.iter_mut().zip([1,4,8]) { w.name=n.to_string(); w.num=Some(n); }
        let ids: Vec<_> = o.workspaces.iter().map(|w| w.id).collect();
        let name = o.name.clone();
        let plan = rename_plan(&t,&name,&[ids[2],ids[0],ids[1]]).unwrap();
        assert_eq!(plan, vec![(ids[2],"8".into(),"1".into()),(ids[0],"1".into(),"4".into()),(ids[1],"4".into(),"8".into())]);
        assert!(rename_plan(&t,&name,&ids).unwrap().is_empty());
        let other = t.clone();
        assert!(same_topology(&t,&other));
        t.outputs[0].workspaces.swap(0,1);
        assert!(!same_topology(&t,&other));
    }
}
