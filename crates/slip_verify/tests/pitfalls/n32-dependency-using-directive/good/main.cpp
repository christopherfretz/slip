#include "dep.h"

namespace acme {
int make(int n) { return n; }
}
namespace legacy { using acme::make; }
#include "client.h"
