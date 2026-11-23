namespace legacy { namespace detail { int a; int b; } }
int use() { return legacy::detail::a + legacy::detail::b; }
