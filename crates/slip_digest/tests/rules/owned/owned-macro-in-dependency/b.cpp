#define MAKE(N) struct N { int v; };
#include "dep.h"
namespace acme { struct Mine {}; }
int use() { return sizeof(legacy::Made) + sizeof(acme::Mine); }
