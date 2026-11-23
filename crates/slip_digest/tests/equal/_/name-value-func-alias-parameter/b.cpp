// WP-104 (digest §9), __func__ holds the unqualified name only: a parameter respelled through an alias
struct Widget {};
using Handle = Widget;
const char *f(Handle) { return __func__; }
const char *g(Handle) { return __FUNCTION__; }
