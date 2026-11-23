namespace legacy { namespace detail { int a; } }
namespace acme { namespace detail { int b; } }
int use() { return legacy::detail::a + acme::detail::b; }
