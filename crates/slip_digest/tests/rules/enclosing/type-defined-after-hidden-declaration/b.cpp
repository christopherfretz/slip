namespace __attribute__((visibility("hidden"))) n { struct S; }
template <class T> void g() {}
inline void use1() { g<n::S *>(); }
namespace __attribute__((visibility("hidden"))) n { struct S { virtual ~S() {} }; }
void use2() { use1(); n::S s; }
