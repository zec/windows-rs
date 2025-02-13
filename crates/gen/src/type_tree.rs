use super::*;

#[derive(Debug)]
pub struct TypeTree {
    pub namespace: &'static str,
    pub types: Vec<ElementType>,
    pub namespaces: BTreeMap<&'static str, TypeTree>,
}

impl TypeTree {
    fn from_namespace(namespace: &'static str) -> Self {
        Self {
            namespace,
            types: Vec::new(),
            namespaces: BTreeMap::new(),
        }
    }

    pub fn from_limits(reader: &'static TypeReader, limits: &TypeLimits) -> Self {
        let mut root = Self::from_namespace("");

        let mut set = BTreeSet::new();

        for limit in limits.limits() {
            match &limit.limit {
                TypeLimit::All => {
                    for def in reader.namespace_types(&limit.namespace) {
                        root.insert_if(reader, &limit.namespace, &mut set, &def);
                    }
                }
                TypeLimit::Some(types) => {
                    for name in types {
                        root.insert_if(
                            reader,
                            &limit.namespace,
                            &mut set,
                            &reader.resolve_type(&limit.namespace, name),
                        );
                    }
                }
            }
        }

        root
    }

    fn insert_if(
        &mut self,
        reader: &TypeReader,
        namespace: &'static str,
        set: &mut BTreeSet<Row>,
        t: &ElementType,
    ) {
        if set.insert(t.row()) {
            for def in t.dependencies() {
                self.insert_if(reader, def.namespace(), set, &def);
            }

            if !namespace.is_empty() {
                self.insert(namespace, 0, t);
            }
        }
    }

    fn insert(&mut self, namespace: &'static str, pos: usize, t: &ElementType) {
        if let Some(next) = namespace[pos..].find('.') {
            let next = pos + next;
            self.namespaces
                .entry(&namespace[pos..next])
                .or_insert_with(|| Self::from_namespace(&namespace[..next]))
                .insert(namespace, next + 1, t);
        } else {
            self.namespaces
                .entry(&namespace[pos..])
                .or_insert_with(|| Self::from_namespace(namespace))
                .types
                .push(t.clone());
        }
    }

    pub fn remove(&mut self, namespace: &str) {
        if let Some(pos) = namespace.find('.') {
            if let Some(tree) = self.namespaces.get_mut(&namespace[..pos]) {
                tree.remove(&namespace[pos + 1..])
            }
        } else {
            self.namespaces.remove(namespace);
        }
    }

    pub fn gen<'a>(&'a self) -> impl Iterator<Item = TokenStream> + 'a {
        let gen = Gen::Relative(self.namespace);

        self.types
            .iter()
            .map(move |t| t.gen(gen))
            .chain(gen_namespaces(&self.namespaces))
    }
}

fn gen_namespaces<'a>(
    namespaces: &'a BTreeMap<&'static str, TypeTree>,
) -> impl Iterator<Item = TokenStream> + 'a {
    namespaces.iter().map(|(name, tree)| {
        let name = to_snake(name);
        let name = to_ident(&name);

        let tokens = tree.gen();

        quote! {
            // TODO: remove `unused_variables` when https://github.com/microsoft/windows-rs/issues/212 is fixed
            #[allow(unused_variables, non_upper_case_globals, non_snake_case)]
            pub mod #name {
                #(#tokens)*
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tree() {
        let reader = TypeReader::get();
        let mut limits = TypeLimits::new(reader);

        limits
            .insert(NamespaceTypes {
                namespace: "Windows.Win32.FileSystem",
                limit: TypeLimit::Some(vec!["FILE_ACCESS_FLAGS".to_string()]),
            })
            .unwrap();

        let tree = TypeTree::from_limits(reader, &limits);

        assert_eq!(tree.namespace, "");
        assert_eq!(tree.types.len(), 0);
        assert_eq!(tree.namespaces.len(), 1);

        let tree = &tree.namespaces["Windows"];

        assert_eq!(tree.namespace, "Windows");
        assert_eq!(tree.types.len(), 0);
        assert_eq!(tree.namespaces.len(), 1);

        let tree = &tree.namespaces["Win32"];

        assert_eq!(tree.namespace, "Windows.Win32");
        assert_eq!(tree.types.len(), 0);
        assert_eq!(tree.namespaces.len(), 1);

        let tree = &tree.namespaces["FileSystem"];

        assert_eq!(tree.namespace, "Windows.Win32.FileSystem");
        assert_eq!(tree.types.len(), 1);
        assert_eq!(tree.namespaces.len(), 0);

        let t = &tree.types[0];
        assert_eq!(
            t.gen_name(Gen::Absolute).as_str(),
            "windows :: win32 :: file_system :: FILE_ACCESS_FLAGS"
        );
    }
}
