namespace legacy {
namespace io { struct out {}; }
template <class T> io::out &operator<<(io::out &o, const T &) { return o; }
template <class T> void log(io::out &o, const T &t) { o << t; }
}
