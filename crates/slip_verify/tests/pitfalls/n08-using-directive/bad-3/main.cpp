#include "dep.h"

namespace acme {
int make(int) { return 1; }
}

using namespace legacy;

#include "client.h"
