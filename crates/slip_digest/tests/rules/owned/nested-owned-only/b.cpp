namespace acme { namespace detail { struct Mine {}; } }
int use() { return sizeof(acme::detail::Mine); }
