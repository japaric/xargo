// TODO de-unwrap-ify

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use daggy::{Dag, NodeIndex, Walker};
use serde_json::Value;
use serde_json;

use CommandExt;
use errors::*;

/// Dependency graph of the `std` crate
pub struct DependencyGraph {
    dag: Dag<String, ()>,
}

impl DependencyGraph {
    pub fn new() -> Self {
        DependencyGraph { dag: Dag::new() }
    }

    pub fn add_edge(&mut self, parent: &str, child: &str) {
        let parent = self.add_node(parent);

        if let Some(child) = self.get_node(child) {
            self.dag.add_edge(parent, child, ()).unwrap();
            return;
        }

        self.dag.add_child(parent, (), child.to_owned());
    }

    /// Tries to compile as much of the dependency graph as possible
    pub fn compile<F>(&self, mut f: F) -> Result<()>
        where F: FnMut(&str) -> Result<bool>
    {
        let mut failures = vec![];
        let mut successes = vec![];

        self.compile_(self.get_node("std").unwrap(),
                      &mut successes,
                      &mut failures,
                      &mut f)
            .map(|_| {})
    }

    fn compile_<F>(&self,
                   pkg: NodeIndex,
                   successes: &mut Vec<NodeIndex>,
                   failures: &mut Vec<NodeIndex>,
                   f: &mut F)
                   -> Result<bool>
        where F: FnMut(&str) -> Result<bool>
    {
        let mut at_least_one_dep_failed = false;

        for (_, child) in self.dag.children(pkg).iter(&self.dag) {
            if !self.compile_(child, successes, failures, f)? {
                at_least_one_dep_failed = true;
            }
        }

        if at_least_one_dep_failed {
            failures.push(pkg);
            Ok(false)
        } else if failures.contains(&pkg) {
            Ok(false)
        } else if successes.contains(&pkg) {
            Ok(true)
        } else {
            if f(self.name_of(pkg))? {
                successes.push(pkg);
                Ok(true)
            } else {
                failures.push(pkg);
                Ok(false)
            }
        }
    }

    fn add_node(&mut self, pkg: &str) -> NodeIndex {
        if let Some(i) = self.dag
            .raw_nodes()
            .iter()
            .enumerate()
            .filter(|&(_, ref node)| node.weight == pkg)
            .map(|(i, _)| i)
            .next() {
            return NodeIndex::new(i);
        }

        self.dag.add_node(pkg.to_owned());

        NodeIndex::new(self.dag.raw_nodes().len() - 1)
    }

    fn get_node(&self, pkg: &str) -> Option<NodeIndex> {
        self.dag
            .raw_nodes()
            .iter()
            .enumerate()
            .filter(|&(_, ref node)| node.weight == pkg)
            .map(|(i, _)| NodeIndex::new(i))
            .next()
    }

    fn name_of(&self, idx: NodeIndex) -> &str {
        &self.dag.raw_nodes()[idx.index()].weight
    }
}


/// Builds the dependency graph of the `std` crate
pub fn build(rust_src: &Path) -> Result<DependencyGraph> {
    let metadata: Value = try!(serde_json::from_str(&try!(Command::new("cargo")
            .arg("metadata")
            .arg("--all-features")
            .arg("--manifest-path")
            .arg(rust_src.join("src/libstd/Cargo.toml"))
            .run_and_get_stdout()))
        .chain_err(|| "couldn't parse the output of `cargo metadata`"));

    let package = metadata.pointer("/packages")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p.pointer("/name").unwrap().as_str().unwrap(), p))
        .collect::<HashMap<_, _>>();

    let mut to_visit = vec![package["std"]];
    let mut visited = HashSet::new();

    let mut dg = DependencyGraph::new();
    while let Some(current_package) = to_visit.pop() {
        let parent =
            current_package.pointer("/name").unwrap().as_str().unwrap();

        visited.insert(parent);

        let deps = current_package.pointer("/dependencies")
            .unwrap()
            .as_array()
            .unwrap();

        for dep in deps {
            if dep.pointer("/kind").and_then(|v| v.as_str()) != Some("build") {
                let child = dep.pointer("/name").unwrap().as_str().unwrap();

                if !visited.contains(&child) {
                    to_visit.push(&package[child]);
                }

                dg.add_edge(parent, child);
            }
        }
    }

    Ok(dg)
}
