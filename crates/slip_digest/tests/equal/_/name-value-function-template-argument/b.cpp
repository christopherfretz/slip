// WP-104 (digest §9), passed to a function template taking a pointer
template <class T> void tt(T, const char *);
template <class T> void deduced(T);
namespace acme {
void f() { tt(1, __PRETTY_FUNCTION__); deduced(__PRETTY_FUNCTION__); }
}
