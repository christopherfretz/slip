namespace __attribute__((visibility("hidden"))) t { struct S { virtual ~S(); }; }
namespace __attribute__((visibility("hidden"), type_visibility("hidden"))) d { }
t::S::~S() {}
