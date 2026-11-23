namespace my { struct T {}; }
namespace __attribute__((visibility("hidden"))) lib { template <class U> struct S { virtual void f() {} }; }
namespace lib { template <> struct S<my::T> { virtual void f() {} }; }
const void* u() { lib::S<my::T> s; s.f(); return &s; }
