// WP-104 (digest §9), __func__ holds the unqualified name only: a parameter respelled through an alias
struct Widget {};
using Handle = Widget;
const char *f(Widget) { return __func__; }
const char *g(Widget) { return __FUNCTION__; }
