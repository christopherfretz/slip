#include "side.h"
// WP-050: Kiln declares `__builtin_va_list` without a location in every
// translation unit; a program's typedef redeclares it and is a declared
// unit of this file (digest §11 origin file). Its record
// (`__va_list_tag`) is invisible to lookup: a derived unit the
// typedef refers to (§2.2, a part of a builtin).
#ifdef B
typedef __builtin_va_list __builtin_va_list;
#endif
int x;
