#pragma pack(push, 1)
#include "a.h"
#include "b.h"
#pragma pack(pop)


int sizes() { return sizeof(A) + sizeof(B); }
