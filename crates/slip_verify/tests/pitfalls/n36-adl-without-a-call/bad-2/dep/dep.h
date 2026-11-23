namespace legacy {
namespace util {
struct Tag {};
template <class T> int *begin(T &) { return nullptr; }
template <class T> int *end(T &) { return nullptr; }
}
}
