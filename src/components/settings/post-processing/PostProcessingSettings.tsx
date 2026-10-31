import React from "react";
import { useTranslation } from "react-i18next";

import { SettingsGroup, ToggleSwitch } from "@/components/ui";
import { useSettings } from "../../../hooks/useSettings";

export const PostProcessingSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const enabled = getSetting("post_process_enabled") ?? false;
  const hasGeminiKey = getSetting("gemini_api_key_set") ?? false;

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.postProcessing.title")}>
        <ToggleSwitch
          checked={enabled}
          onChange={(checked) => updateSetting("post_process_enabled", checked)}
          isUpdating={isUpdating("post_process_enabled")}
          label={t("settings.postProcessing.cleanup.label")}
          description={t("settings.postProcessing.cleanup.description")}
          descriptionMode="inline"
          grouped={true}
        />
      </SettingsGroup>
      {!hasGeminiKey && (
        <div className="p-3 bg-mid-gray/5 rounded-md border border-mid-gray/20">
          <p className="text-sm text-mid-gray">
            {t("settings.postProcessing.cleanup.missingKey")}
          </p>
        </div>
      )}
    </div>
  );
};
