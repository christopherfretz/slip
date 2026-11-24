// WP-104 (digest §9), __PRETTY_FUNCTION__ holds the return type as written
struct Widget {};
using Handle = Widget;
Widget f(const char *&p) { p = __PRETTY_FUNCTION__; return {}; }
