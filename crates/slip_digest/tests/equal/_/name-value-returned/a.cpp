// WP-104 (digest §9), returned
namespace legacy {
const char *f() { return __PRETTY_FUNCTION__; }
const char *g() { return (__func__); }
}
