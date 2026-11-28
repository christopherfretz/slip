#include "side.h"
#include "new_dep.h"
#include "ro_inc.h"
#include "own_dir.h"
#ifdef B
namespace acme { int g() { return 1; } }
template <class T> T f(T t) { return acme::forward<T>(t); }
#else
namespace legacy { int g() { return 1; } }
template <class T> T f(T t) { return legacy::forward<T>(t); }
#endif
