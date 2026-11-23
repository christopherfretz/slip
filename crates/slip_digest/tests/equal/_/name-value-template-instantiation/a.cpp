// WP-104 (digest §9), in an explicitly instantiated function template
namespace legacy {
template <class T> const char *g(T) { return __PRETTY_FUNCTION__; }
template const char *g<int>(int);
const char *f() { return g(1); }
}
