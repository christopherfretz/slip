#include "dep.h"

namespace {
int helper() { return 1; }
}
namespace acme {
int api() { return helper(); }
}

int user() { return acme::api(); }
