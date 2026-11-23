#include "dep.h"
int helper(long) { return 2; }
namespace legacy {

template <class T> int use(T t) { return helper(t); }
}

int caller() { return legacy::use(0); }
