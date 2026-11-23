#include "dep.h"
const int limit = 4;
namespace acme {
struct Box { int n = legacy::limit; };
}

int caller() { return acme::Box().n; }
