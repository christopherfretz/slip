namespace __attribute__((visibility("hidden"), type_visibility("default"))) t { }
namespace __attribute__((visibility("hidden"))) d { struct S { virtual ~S(); }; }
d::S::~S() {}
