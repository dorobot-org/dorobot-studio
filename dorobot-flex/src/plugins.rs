//! 修改这一处组合清单即可接入另一个独立 Rust 插件包。
use dorobot_studio_plugin::PluginRegistry;
pub fn builtin_plugins() -> PluginRegistry {
    let mut registry = PluginRegistry::default();
    registry
        .register(Box::new(dorobot_plugin_control::ControlPlugin))
        .expect("控制插件注册失败");
    registry
        .register(Box::new(dorobot_plugin_collection::CollectionPlugin))
        .expect("采集插件注册失败");
    registry
        .register(Box::new(dorobot_plugin_policy::PolicyPlugin))
        .expect("训练与推理插件注册失败");
    registry.register(Box::new(dorobot_plugin_aisle::AislePlugin)).expect("AISLE plugin registration");
    registry
}
