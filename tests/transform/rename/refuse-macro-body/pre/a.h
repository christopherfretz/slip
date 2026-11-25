#pragma once
namespace a {
template <class T> int f() { return sizeof(T); }
}
#define REG(T) int reg_##T() { return a::f<T>(); }
#define REG2(T) int reg2_##T() { return ::a::f<T>() + 1; }
