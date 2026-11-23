#include "dep.h"

namespace legacy {
namespace {
int helper() { return 1; }
}
int api() { return helper(); }
}

int user() { return legacy::api(); }
