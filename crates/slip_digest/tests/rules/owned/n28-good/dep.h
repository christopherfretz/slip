namespace legacy {
namespace io { struct out {}; }

template <class T> void log(io::out &o, const T &t) { o << t; }
}
