namespace acme { namespace detail { int x() { return 1; } } }
namespace legacy { namespace detail { int g() { return 2; } } }
int u() { return acme::detail::x() + legacy::detail::g(); }
