namespace legacy { int a; namespace detail { int d; int e; } }
int u() { return legacy::a + legacy::detail::d + legacy::detail::e; }
