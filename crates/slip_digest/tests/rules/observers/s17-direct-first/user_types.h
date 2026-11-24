#pragma once
#ifdef POST
namespace fresh { struct UserData { int v; }; }
namespace old_cfg { typedef fresh::UserData data_t; }
#else
namespace old { struct UserData { int v; }; }
namespace old_cfg { typedef old::UserData data_t; }
#endif
