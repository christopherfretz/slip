#include "dep.h"

namespace acme {
using legacy::helper;
template <class T> int use(T t) { return helper(t); }
}

int caller() { return acme::use(0); }
