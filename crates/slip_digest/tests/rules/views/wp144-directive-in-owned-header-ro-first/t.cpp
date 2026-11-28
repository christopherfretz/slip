#include "side.h"
#include "new_dep.h"
#include "ro_inc.h"
#include "own_dir.h"
#ifdef B
template <class T> T f(T t) { return acme::forward<T>(t); }
#else
template <class T> T f(T t) { return legacy::forward<T>(t); }
#endif
