// WP-104 (digest §9), in an implicitly instantiated function template (the name holds [T = long])
namespace legacy {
template <class T> const char *g(T) { return __PRETTY_FUNCTION__; }
const char *f() { return g(1L); }
}
