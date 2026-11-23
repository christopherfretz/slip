#include "dep.h"
#include "acme.h"

void helper(int) {}

namespace legacy {
void caller() { helper(1); }
}
