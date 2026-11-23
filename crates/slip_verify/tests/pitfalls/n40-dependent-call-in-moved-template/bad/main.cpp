#include "dep.h"

namespace acme {

template <class T> int use(T t) { return helper(t); }
}

int caller() { return acme::use(0); }
