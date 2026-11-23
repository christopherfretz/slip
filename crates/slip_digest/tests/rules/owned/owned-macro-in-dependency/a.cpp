#define MAKE(N) struct N { int v; };
#include "dep.h"
namespace legacy { struct Mine {}; }
int use() { return sizeof(legacy::Made) + sizeof(legacy::Mine); }
