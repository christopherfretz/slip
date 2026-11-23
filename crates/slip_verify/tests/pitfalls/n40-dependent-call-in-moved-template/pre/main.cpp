#include "dep.h"

namespace legacy {

template <class T> int use(T t) { return helper(t); }
}

int caller() { return legacy::use(0); }
