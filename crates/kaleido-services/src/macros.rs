//! Macros to reduce boilerplate across service implementations.
//!
//! Eliminates the repetitive `plugin()` function pattern and the dead
//! `ctx: Context` field that every service used to carry.

/// Generates the `plugin()` registration function for a service.
///
/// Before:
/// ```ignore
/// pub fn plugin() -> PluginHandle {
///     service_sync::<XxxServiceImpl, (), _>(
///         "xxx_service",
///         Inject::new(["dep1", "dep2"]),
///         |ctx, _config| {
///             let dep1 = ctx.require::<Arc<dyn Dep1>>("dep1")?;
///             let dep2 = ctx.require::<Arc<dyn Dep2>>("dep2")?;
///             Ok(XxxServiceImpl::new(dep1, dep2))
///         },
///     )
/// }
/// ```
///
/// After:
/// ```ignore
/// service_plugin!(XxxServiceImpl, "xxx_service",
///     deps: ["dep1", "dep2"],
///     build: |ctx| {
///         let dep1 = ctx.require::<Arc<dyn Dep1>>("dep1")?;
///         let dep2 = ctx.require::<Arc<dyn Dep2>>("dep2")?;
///         Ok(XxxServiceImpl::new(dep1, dep2))
///     }
/// );
/// ```
///
/// The `deps` list auto-generates the `Inject::new([...])` call.
#[macro_export]
macro_rules! service_plugin {
    ($impl:ty, $id:literal, deps: [$($dep:literal),* $(,)?], build: $build:expr $(,)?) => {
        pub fn plugin() -> ::cordis::PluginHandle {
            ::cordis::service_sync::<$impl, (), _>(
                $id,
                ::cordis::Inject::new([$($dep),*]),
                $build,
            )
        }
    };
    // Variant with Inject::none()
    ($impl:ty, $id:literal, deps: none, build: $build:expr $(,)?) => {
        pub fn plugin() -> ::cordis::PluginHandle {
            ::cordis::service_sync::<$impl, (), _>(
                $id,
                ::cordis::Inject::none(),
                $build,
            )
        }
    };
}

/// Shorthand for the `impl Service for Xxx { const NAME }` boilerplate.
///
/// Before:
/// ```ignore
/// impl Service for DataServiceImpl {
///     const NAME: &'static str = "data_service";
/// }
/// ```
///
/// After:
/// ```ignore
/// impl_service!(DataServiceImpl, "data_service");
/// ```
#[macro_export]
macro_rules! impl_service {
    ($impl:ty, $id:literal) => {
        impl ::cordis::Service for $impl {
            const NAME: &'static str = $id;
        }
    };
}
