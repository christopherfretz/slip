#include "dep.h"

namespace acme {
int make(int) { return 1; }
}

using namespace legacy;
using namespace acme;
#include "client.h"
