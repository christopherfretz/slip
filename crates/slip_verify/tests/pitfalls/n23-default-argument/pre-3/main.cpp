#include "dep.h"
const int limit = 4;
namespace legacy {
struct Box { int n = limit; };
}

int caller() { return legacy::Box().n; }
