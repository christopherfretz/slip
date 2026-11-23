#include "dep.h"

namespace acme {
int run(int n) { return n + 1; }
}
namespace legacy { using acme::run; }
int caller() { return LEGACY_CALL(run, 1); }
