namespace n { template <class T> struct S { virtual void f() {} }; }
namespace __attribute__((visibility("hidden"))) n { }
const void* u() { n::S<int> s; s.f(); return &s; }
