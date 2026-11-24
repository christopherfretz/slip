namespace legacy { inline namespace in { int h(int); int h(long); } }
template <class T> int f(T t) { return legacy::h(t); }
namespace zzz { int z; }
