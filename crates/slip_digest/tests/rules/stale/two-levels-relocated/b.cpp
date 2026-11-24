namespace acme { namespace impl { int d; } }
namespace legacy { namespace detail { int k; } }
int u() { return acme::impl::d + legacy::detail::k; }
