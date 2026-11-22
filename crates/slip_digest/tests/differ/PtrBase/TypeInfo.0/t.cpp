#include "side.h"
// §8 PtrBase `TypeInfo.0`: `&typeid(int)` against `&typeid(char)`.
namespace std { class type_info; }
#ifdef B
constexpr const std::type_info* p = &typeid(char);
#else
constexpr const std::type_info* p = &typeid(int);
#endif
