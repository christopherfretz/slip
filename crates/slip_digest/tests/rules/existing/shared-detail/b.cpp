namespace acme { namespace detail { int x() { return 1; } } }
namespace acme { namespace detail { int g() { return 2; } } }
int u() { return acme::detail::x() + acme::detail::g(); }
