namespace legacy {
template <class T> struct traits {};
template <class T> constexpr int width = 0;
template <class T> int id(T) { return 0; }
namespace lib { template <class T> struct hash {}; }
}
