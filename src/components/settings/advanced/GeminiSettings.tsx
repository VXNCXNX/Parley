import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../../hooks/useSettings";
import { commands } from "@/bindings";
import { SettingContainer } from "../../ui/SettingContainer";
import { Dropdown } from "../../ui/Dropdown";
import { Input } from "../../ui/Input";

const GEMINI_MODELS = [
  { value: "gemini-2.5-flash", label: "Gemini 2.5 Flash" },
  { value: "gemini-2.5-pro", label: "Gemini 2.5 Pro" },
  { value: "gemini-3-flash-preview", label: "Gemini 3 Flash" },
  { value: "gemini-3.1-flash-lite-preview", label: "Gemini 3.1 Flash Lite" },
  { value: "chirp_3", label: "Chirp 3 (dedicated STT, fastest)" },
];

const CHIRP_LOCATIONS = [
  { value: "europe-west2", label: "europe-west2 (London)" },
  { value: "europe-west3", label: "europe-west3 (Frankfurt)" },
  { value: "northamerica-northeast1", label: "northamerica-northeast1 (Montreal)" },
  { value: "asia-south1", label: "asia-south1 (Mumbai)" },
];

export const GeminiSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, refreshSettings } = useSettings();

  const hasApiKey = !!(getSetting("gemini_api_key_set") as boolean | undefined);
  const hasServiceAccount = !!(getSetting("chirp_service_account_set") as
    | boolean
    | undefined);
  const currentModel =
    (getSetting("gemini_model") as string | undefined) ?? "gemini-2.5-flash";
  const location =
    (getSetting("gemini_location") as string | undefined) ?? "europe-west2";
  const [localApiKey, setLocalApiKey] = useState("");
  const [localServiceAccount, setLocalServiceAccount] = useState("");
  const [saStatus, setSaStatus] = useState<"idle" | "saving" | "saved" | "error">(
    "idle",
  );
  const [saError, setSaError] = useState<string>("");

  const isChirp = currentModel.startsWith("chirp");

  const handleApiKeyBlur = async () => {
    if (localApiKey) {
      await commands.changeGeminiApiKeySetting(localApiKey);
      await refreshSettings();
      setLocalApiKey("");
    }
  };

  const saveServiceAccount = async () => {
    if (!localServiceAccount) return;
    setSaStatus("saving");
    setSaError("");
    const result = await commands.changeChirpServiceAccountSetting(
      localServiceAccount,
    );
    if (result.status === "ok") {
      setSaStatus("saved");
      setLocalServiceAccount("");
      await refreshSettings();
      setTimeout(() => setSaStatus("idle"), 2000);
    } else {
      setSaStatus("error");
      setSaError(result.error);
    }
  };

  return (
    <>
      <SettingContainer
        title={t("settings.gemini.apiKey")}
        description={t("settings.gemini.description")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <div className="flex items-center justify-end gap-2">
          <Input
            type="password"
            value={localApiKey}
            onChange={(e) => setLocalApiKey(e.target.value)}
            onBlur={handleApiKeyBlur}
            placeholder={hasApiKey ? "********" : t("settings.gemini.apiKeyPlaceholder")}
            variant="compact"
            className="flex-1 w-[280px]"
          />
        </div>
      </SettingContainer>

      <SettingContainer
        title={t("settings.gemini.model")}
        description={t("settings.gemini.modelDescription")}
        descriptionMode="tooltip"
        layout="horizontal"
        grouped={true}
      >
        <div className="flex items-center justify-end gap-2">
          <Dropdown
            options={GEMINI_MODELS}
            selectedValue={currentModel}
            onSelect={(value) => updateSetting("gemini_model", value)}
            className="w-[280px]"
          />
        </div>
      </SettingContainer>

      {isChirp && (
        <>
          <SettingContainer
            title="Service Account JSON"
            description="Paste the full content of the service account JSON key file. Required for Chirp 3."
            descriptionMode="tooltip"
            layout="stacked"
            grouped={true}
          >
            <div className="flex flex-col gap-2 w-full">
              <textarea
                value={localServiceAccount}
                onChange={(e) => setLocalServiceAccount(e.target.value)}
                placeholder={
                  hasServiceAccount
                    ? "{ ...service account configured (paste new JSON to replace)... }"
                    : '{ "type": "service_account", "project_id": "...", ... }'
                }
                rows={6}
                className="w-full text-xs font-mono bg-input border border-border rounded p-2 resize-y"
              />
              <div className="flex items-center gap-2">
                <button
                  onClick={saveServiceAccount}
                  disabled={!localServiceAccount || saStatus === "saving"}
                  className="px-3 py-1 text-sm rounded bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                >
                  {saStatus === "saving" ? "Saving..." : "Save Service Account"}
                </button>
                {saStatus === "saved" && (
                  <span className="text-sm text-green-500">Saved ✓</span>
                )}
                {saStatus === "error" && (
                  <span className="text-sm text-red-500">{saError}</span>
                )}
                {hasServiceAccount && saStatus === "idle" && (
                  <span className="text-sm text-muted-foreground">
                    Service account configured ✓
                  </span>
                )}
              </div>
            </div>
          </SettingContainer>

          <SettingContainer
            title="Chirp 3 region"
            description="Chirp 3 is only available in these regions."
            descriptionMode="tooltip"
            layout="horizontal"
            grouped={true}
          >
            <div className="flex items-center justify-end gap-2">
              <Dropdown
                options={CHIRP_LOCATIONS}
                selectedValue={location}
                onSelect={(value) =>
                  commands.changeGeminiLocationSetting(value).then(refreshSettings)
                }
                className="w-[280px]"
              />
            </div>
          </SettingContainer>
        </>
      )}
    </>
  );
};
