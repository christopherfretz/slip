#include "new_dep.h"
namespace acme { int g(int); int g(long); int k(int); int k(long); int m(int); int m(long); }
template <class T> int f(T t) { return acme::g(t) + acme::k(t) + acme::m(t); }
