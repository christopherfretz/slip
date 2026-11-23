#include "base.h"
namespace legacy { struct Late; }
#include "dep.h"
int use() { return sizeof(legacy::Late); }
