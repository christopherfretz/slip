#include "dep.h"

namespace acme {
namespace {
inline int helper() { return 1; }
int base() { return 2; }
}
int api() { return helper() + base(); }
}

int user() { return acme::api(); }
