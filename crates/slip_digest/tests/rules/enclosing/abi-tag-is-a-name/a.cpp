inline namespace __attribute__((abi_tag("x"))) a { struct S { }; }
namespace d { }
S g() { return S(); }
