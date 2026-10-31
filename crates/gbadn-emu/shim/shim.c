/* Narrow C interface over mGBA: struct mCore's layout stays out of Rust. */
#include <mgba/flags.h>

#include <fcntl.h>
#include <stdlib.h>

#include <mgba/core/blip_buf.h>
#include <mgba/core/config.h>
#include <mgba/core/core.h>
#include <mgba/core/log.h>
#include <mgba-util/vfs.h>

#define GBA_WIDTH 240
#define GBA_HEIGHT 160

enum {
	GBADN_OK = 0,
	GBADN_NOT_A_GBA_ROM = 1,
	GBADN_CORE_FAILED = 2,
};

struct gbadn_core {
	struct mCore* core;
	color_t video[GBA_WIDTH * GBA_HEIGHT];
};

static void _silentLog(struct mLogger* logger, int category, enum mLogLevel level, const char* format, va_list args) {
	UNUSED(logger);
	UNUSED(category);
	UNUSED(level);
	UNUSED(format);
	UNUSED(args);
}

static struct mLogger _silentLogger = {
	.log = _silentLog,
};

struct gbadn_core* gbadn_core_new(const uint8_t* rom, size_t size, const char* save_path, int* status) {
	/* mGBA logs every emulated hardware event to the terminal unless told otherwise. */
	mLogSetDefaultLogger(&_silentLogger);

	/* The ROM stays in memory: this copy is owned by the core and is never written to disk. */
	struct VFile* vf = VFileMemChunk(rom, size);
	if (!vf) {
		*status = GBADN_CORE_FAILED;
		return NULL;
	}
	if (mCoreIsCompatible(vf) != mPLATFORM_GBA) {
		vf->close(vf);
		*status = GBADN_NOT_A_GBA_ROM;
		return NULL;
	}

	struct gbadn_core* handle = calloc(1, sizeof(*handle));
	struct mCore* core = mCoreCreate(mPLATFORM_GBA);
	if (!handle || !core || !core->init(core)) {
		free(handle);
		free(core);
		vf->close(vf);
		*status = GBADN_CORE_FAILED;
		return NULL;
	}
	handle->core = core;

	mCoreInitConfig(core, NULL);
	struct mCoreOptions options = {
		.useBios = false,
		.audioBuffers = 2048,
		.volume = 0x100,
	};
	mCoreConfigLoadDefaults(&core->config, &options);
	mCoreLoadForeignConfig(core, &core->config);

	core->setVideoBuffer(core, handle->video, GBA_WIDTH);
	core->setAudioBufferSize(core, 2048);

	if (!core->loadROM(core, vf)) {
		vf->close(vf);
		mCoreConfigDeinit(&core->config);
		core->deinit(core);
		free(handle);
		*status = GBADN_CORE_FAILED;
		return NULL;
	}

	if (save_path) {
		struct VFile* save = VFileOpen(save_path, O_CREAT | O_RDWR);
		if (save) {
			core->loadSave(core, save);
		}
	}

	core->reset(core);
	*status = GBADN_OK;
	return handle;
}

void gbadn_core_free(struct gbadn_core* handle) {
	mCoreConfigDeinit(&handle->core->config);
	handle->core->deinit(handle->core);
	free(handle);
}

void gbadn_core_run_frame(struct gbadn_core* handle, uint32_t keys) {
	handle->core->setKeys(handle->core, keys);
	handle->core->runFrame(handle->core);
}

size_t gbadn_core_state_size(struct gbadn_core* handle) {
	return handle->core->stateSize(handle->core);
}

bool gbadn_core_save_state(struct gbadn_core* handle, void* state) {
	return handle->core->saveState(handle->core, state);
}

bool gbadn_core_load_state(struct gbadn_core* handle, const void* state) {
	return handle->core->loadState(handle->core, state);
}

const uint32_t* gbadn_core_video(const struct gbadn_core* handle) {
	return handle->video;
}

size_t gbadn_core_read_audio(struct gbadn_core* handle, int16_t* stereo, size_t max_frames, int rate) {
	struct mCore* core = handle->core;
	struct blip_t* left = core->getAudioChannel(core, 0);
	struct blip_t* right = core->getAudioChannel(core, 1);
	blip_set_rates(left, core->frequency(core), rate);
	blip_set_rates(right, core->frequency(core), rate);

	size_t available = blip_samples_avail(left);
	size_t frames = available < max_frames ? available : max_frames;
	blip_read_samples(left, stereo, (int) frames, 1);
	blip_read_samples(right, stereo + 1, (int) frames, 1);
	return frames;
}
