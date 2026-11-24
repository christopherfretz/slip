namespace acme { int a; }
namespace det { int d; }
namespace legacy { namespace detail { int e; } }
int u() { return acme::a + det::d + legacy::detail::e; }
