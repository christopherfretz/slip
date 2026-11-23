#include "dep.h"
const int limit = 4;
namespace acme {
struct Box { int n = limit; };
}

int caller() { return acme::Box().n; }
