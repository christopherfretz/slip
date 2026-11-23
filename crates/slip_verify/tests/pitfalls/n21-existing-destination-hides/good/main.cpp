#include "dep.h"
#include "acme.h"

void helper(int) {}

namespace acme {
void caller() { ::helper(1); }
}
