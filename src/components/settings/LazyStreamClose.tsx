import React from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

interface LazyStreamCloseProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const LazyStreamClose: React.FC<LazyStreamCloseProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const enabled =
      (getSetting("lazy_stream_close") as boolean | undefined) ?? false;
    const alwaysOn =
      (getSetting("always_on_microphone") as boolean | undefined) ?? false;
    const timeoutSeconds =
      (getSetting("lazy_stream_close_timeout_seconds") as
        | number
        | undefined) ?? 300;
    const mode = alwaysOn ? "always" : "warm";

    const timeoutOptions = [
      {
        value: "30",
        label: "30s",
      },
      {
        value: "60",
        label: "1 min",
      },
      {
        value: "300",
        label: "5 min",
      },
      {
        value: "600",
        label: "10 min",
      },
    ];

    const handleModeChange = async (nextMode: "warm" | "always") => {
      if (nextMode === "always") {
        await updateSetting("lazy_stream_close", false);
        await updateSetting("always_on_microphone", true);
        return;
      }

      await updateSetting("always_on_microphone", false);
      await updateSetting("lazy_stream_close", true);
    };

    return (
      <SettingContainer
        title={t("settings.advanced.lazyStreamClose.label")}
        description={t("settings.advanced.lazyStreamClose.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-center gap-2">
          <div className="inline-flex rounded-md border border-mid-gray/30 bg-mid-gray/10 p-0.5">
            <button
              type="button"
              className={`px-2 py-1 text-xs font-medium rounded transition-colors ${
                mode === "warm"
                  ? "bg-background-ui text-white"
                  : "hover:bg-logo-primary/10"
              }`}
              disabled={
                isUpdating("always_on_microphone") ||
                isUpdating("lazy_stream_close")
              }
              onClick={() => handleModeChange("warm")}
            >
              {t("settings.advanced.lazyStreamClose.modes.warm")}
            </button>
            <button
              type="button"
              className={`px-2 py-1 text-xs font-medium rounded transition-colors ${
                mode === "always"
                  ? "bg-background-ui text-white"
                  : "hover:bg-logo-primary/10"
              }`}
              disabled={
                isUpdating("always_on_microphone") ||
                isUpdating("lazy_stream_close")
              }
              onClick={() => handleModeChange("always")}
            >
              {t("settings.advanced.lazyStreamClose.modes.always")}
            </button>
          </div>
          <Dropdown
            options={timeoutOptions}
            selectedValue={String(timeoutSeconds)}
            onSelect={(value) =>
              updateSetting(
                "lazy_stream_close_timeout_seconds",
                Number(value),
              )
            }
            disabled={
              mode !== "warm" ||
              !enabled ||
              isUpdating("lazy_stream_close_timeout_seconds")
            }
            className="min-w-[120px]"
          />
        </div>
      </SettingContainer>
    );
  },
);
