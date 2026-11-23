namespace std {
template <class T> struct tuple_size;
template <unsigned long I, class T> struct tuple_element;
}
namespace legacy {
namespace util {
struct Tag {};
template <unsigned long I, class T> int &get(T &) { static int z; return z; }
}
}
