namespace __attribute__((visibility("hidden"), type_visibility("default"))) t { struct S { virtual ~S(); }; }
namespace __attribute__((visibility("hidden"))) d { }
t::S::~S() {}
