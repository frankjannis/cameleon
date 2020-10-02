use super::{elem_name::*, elem_type::*, node_base::*, xml, Parse};

#[derive(Debug, Clone)]
pub struct PortNode {
    attr_base: NodeAttributeBase,
    elem_base: NodeElementBase,

    p_invalidators: Vec<String>,
    chunk_id: Option<ImmOrPNode<u64>>,
    swap_endianness: bool,
    cache_chunk_data: bool,
}

impl PortNode {
    pub fn node_base(&self) -> NodeBase<'_> {
        NodeBase::new(&self.attr_base, &self.elem_base)
    }

    pub fn p_invalidators(&self) -> &[String] {
        &self.p_invalidators
    }

    pub fn chunk_id(&self) -> Option<&ImmOrPNode<u64>> {
        self.chunk_id.as_ref()
    }

    pub fn swap_endianness(&self) -> bool {
        self.swap_endianness
    }

    pub fn cache_chunk_data(&self) -> bool {
        self.cache_chunk_data
    }
}

impl Parse for PortNode {
    fn parse(node: &mut xml::Node) -> Self {
        debug_assert_eq!(node.tag_name(), PORT);

        let attr_base = node.parse();
        let elem_base = node.parse();

        let p_invalidators = node.parse_while(P_INVALIDATOR);
        let chunk_id = if let Some(next_node) = node.next_if(CHUNK_ID) {
            Some(ImmOrPNode::Imm(
                u64::from_str_radix(next_node.text(), 16).unwrap(),
            ))
        } else if let Some(next_node) = node.next_if(P_CHUNK_ID) {
            Some(ImmOrPNode::PNode(next_node.text().into()))
        } else {
            None
        };
        let swap_endianness = node.parse_if(SWAP_ENDIANNESS).unwrap_or_default();
        let cache_chunk_data = node.parse_if(CACHE_CHUNK_DATA).unwrap_or_default();

        Self {
            attr_base,
            elem_base,
            p_invalidators,
            chunk_id,
            swap_endianness,
            cache_chunk_data,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_port_node_with_imm() {
        let xml = r#"
            <Port Name="TestNode">
                <ChunkID>Fd3219</ChunkID>
                <SwapEndianess>Yes</SwapEndianess>
            <Port>
            "#;

        let node: PortNode = xml::Document::from_str(&xml).unwrap().root_node().parse();

        assert_eq!(node.chunk_id().unwrap(), &ImmOrPNode::Imm(0xFd3219));
        assert_eq!(node.swap_endianness(), true);
    }

    #[test]
    fn test_port_node_with_p_node() {
        let xml = r#"
            <Port Name="TestNode">
                <pChunkID>Fd3219</pChunkID>
            <Port>
            "#;

        let node: PortNode = xml::Document::from_str(&xml).unwrap().root_node().parse();

        assert_eq!(
            node.chunk_id().unwrap(),
            &ImmOrPNode::PNode("Fd3219".into())
        );
    }

    #[test]
    fn test_port_node_without_chunk_id() {
        let xml = r#"
            <Port Name="TestNode">
                <CacheChunkData>Yes</CacheChunkData>
            <Port>
            "#;

        let node: PortNode = xml::Document::from_str(&xml).unwrap().root_node().parse();

        assert_eq!(node.chunk_id(), None);
        assert_eq!(node.cache_chunk_data(), true);
    }
}
