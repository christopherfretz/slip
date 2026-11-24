#include "new_dep.h"
namespace legacy { int g(int); int g(long); int k(int); int k(long); int m(int); int m(long); }
template <class T> int f(T t) { return legacy::g(t) + legacy::k(t) + legacy::m(t); }
