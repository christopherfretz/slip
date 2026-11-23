#include "other.h"
namespace acme { struct Mine {}; namespace al = other; using other::thing; using namespace other; }
int use() { acme::thing t; return acme::al::value; }
