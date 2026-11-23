#define Q S<int>::f
namespace n { template <class T> struct S { void f() {} };
template <> void Q() {} }
