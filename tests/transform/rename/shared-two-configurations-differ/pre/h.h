#pragma once
namespace a {
int f(int x);
int f(long x);
}
#ifdef WIDE
typedef long arg_t;
#else
typedef int arg_t;
#endif
inline int call(arg_t v) { return a::f(v); }
