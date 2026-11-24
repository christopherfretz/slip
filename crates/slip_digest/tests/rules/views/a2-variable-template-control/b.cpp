namespace legacy { inline namespace in { int h(int); int h(long); } }
template <class T> int f(T t) { return legacy::in::h(t); }
namespace yyy { int z; }
