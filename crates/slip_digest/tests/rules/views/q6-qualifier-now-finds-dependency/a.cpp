#include "new_dep.h"
namespace legacy { int g(int); int g(long); }
template <class T> int f(T t) { return legacy::g(t); }
