inline namespace __attribute__((abi_tag("x"))) a { }
namespace d { struct S { }; }
d::S g() { return d::S(); }
