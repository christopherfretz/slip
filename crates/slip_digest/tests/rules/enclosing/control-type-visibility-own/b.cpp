namespace __attribute__((visibility("hidden"))) t { }
namespace __attribute__((visibility("hidden"), type_visibility("hidden"))) d { struct S { virtual ~S(); }; }
d::S::~S() {}
