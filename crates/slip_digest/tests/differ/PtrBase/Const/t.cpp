#include "side.h"
// §8 PtrBase `Const`: a pointer to an object with no declaration (`__builtin_source_location`) against a null pointer of its type.
namespace std { struct source_location { struct __impl { const char* _M_file_name; const char* _M_function_name; unsigned _M_line; unsigned _M_column; }; }; }
template <auto V> struct A;
#ifdef B
A<(const std::source_location::__impl*)nullptr>* x;
#else
A<__builtin_source_location()>* x;
#endif
