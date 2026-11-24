#include "new_dep.h"
namespace acme { int g(int); int g(long); }
namespace other { using acme::g; }
template <class T> int f(T t) { return other::g(t); }
