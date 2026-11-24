namespace other { template <class T> int g(T); }
namespace legacy { using other::g; }
template <class T> int f(T t) { return legacy::g(t); }
