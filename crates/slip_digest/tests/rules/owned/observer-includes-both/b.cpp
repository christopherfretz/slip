#include "dep.h"
namespace acme { struct Both { int v; }; }
int use() { return sizeof(acme::Both); }
