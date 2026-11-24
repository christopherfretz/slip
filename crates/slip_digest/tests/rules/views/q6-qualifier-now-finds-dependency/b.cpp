#include "new_dep.h"
namespace acme { int g(int); int g(long); }
template <class T> int f(T t) { return acme::g(t); }
