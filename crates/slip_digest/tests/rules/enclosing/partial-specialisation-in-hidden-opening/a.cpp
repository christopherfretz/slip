namespace n { template <class T> struct P { virtual void f() {} }; }
namespace __attribute__((visibility("hidden"))) n { template <class U> struct P<U *> { virtual void f() {} }; }
const void* u() { n::P<int *> p; p.f(); return &p; }
