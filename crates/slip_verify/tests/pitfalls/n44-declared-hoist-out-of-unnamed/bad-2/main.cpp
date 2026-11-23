#include "dep.h"

namespace acme {
inline int helper() { return 1; }
int api() { return helper(); }
}

int user() { return acme::api(); }
