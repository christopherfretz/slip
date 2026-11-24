// WP-104 (digest §9), __PRETTY_FUNCTION__ holds the parameter types as written: respelled through an alias
struct Widget {};
using Handle = Widget;
const char *f(Handle) { return __PRETTY_FUNCTION__; }
